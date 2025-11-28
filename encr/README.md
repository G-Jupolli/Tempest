# Tempest Encryption

When using raw TCP connections, abstracting out to https seems a bit pointless.  \
Instead, the [Noise](http://www.noiseprotocol.org/) protocol can be used via the `snow` crate.


## Protocol

Uses **Noise_XX_25519_ChaChaPoly_BLAKE2s**:
- XX handshake pattern (mutual authentication)
- Curve25519 for key exchange
- ChaCha20-Poly1305 for encryption
- BLAKE2s for hashing

## Architecture

### Handshake Flow

Client and server independently compute the same shared secret without transmitting it.  \
Noise_XX performs multiple Diffie–Hellman (DH) operations for mutual authentication.  \
The `snow` crate implements DH for us, [wiki](https://en.wikipedia.org/wiki/Diffie%E2%80%93Hellman_key_exchange) for more details.

```
Client                                          Server
  |                                               |
  | Generate ephemeral key pair (e_c)             |
  + - - - - Send e_c public - - - - - - - - - - > |
  |                                               | Generate ephemeral key pair (e_s)
  |                                               | Generate static key pair (s_s)
  |                                               | Compute DH(e_c, e_s) -> ee
  | < - - - Send e_s public, s_s public - - - - - +
  |                                               | Compute DH(e_c, s_s) -> es
  | Generate static key pair (s_c)                |
  | Compute DH(e_c, e_s) -> ee                    |
  | Compute DH(e_c, s_s) -> es                    |
  | Compute DH(s_c, e_s) -> se                    |
  + - - - - Send s_c public - - - - - - - - - - > |
  |                                               | Compute DH(s_c, e_s) -> se
  |                                               |
  | Mix (ee + es + se) -> shared keys             | Mix (ee + es + se) -> shared keys
  
  Client and server now have the correct encryption keys
```

### Lock-Free Encryption

The example of the encrypt / decrypt saw uses `Arc<Mutex<TransportState>>`.  \
However, the challenge of this project is to have 0 locking.  \
To do this, a dedicated green thread handles this without locking to mutate the `Transport`.  

#### Holding channels

The TCP connection is split into the receiving and sending ends.  \
These are both handled on their own green threads and need a channel to the encryption thread.

The client has a static amount of connections but the server has `2c` connections  \
where `c` is the number of clients connected.  \
To just have one method of encryption / decryption the server and client will use  \
  tokio oneshot channels for interactions.  \
This project doesn't validate if this is more / less efficient than an `Arc<Mutex<TransportState>>`.


```
        ┌ - - - - - - - - - - - - - ┐
        v                           |
[ TCP Receiver ] - - ┐              | { oneshot channel response}
                     ┝ - > [ Encryption Handler ]
[  TCP Sender  ] - - ┘              | { oneshot channel response}
        ^                           |
        └ - - - - - - - - - - - - - ┘
```

## Message Flow

### When a client / server needs to send a message

The sequence for sending a message is:
1. Encode with `bincode`
2. Encrypt with `snow` on the dedicated green thread
3. Frame with `tokio_util::codec::LengthDelimitedCodec`
4. Send via TCP

`LengthDelimitedCodec` is applied at the TCP level, tokio will automatically  \
attach the length headers and handle small messages.  \
There is a very tiny overhead over manually implementing this but is more robust.


### When a client / server receives a message

When receiving a message:
1. Receive `tokio_util::codec::LengthDelimitedCodec` framed message
2. Decrypt with `snow` on the dedicated green thread
3. Decode with `bincode`
