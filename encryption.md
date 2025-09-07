So, the plan now is to implement encryption using noise protocol. \
I don't necessarily want to implement the full encryption myself, \
I'll likely use the snow package.

### Basic Idea

My plan is to do it in a similar way to wireguard. \
The client and the server are both going to generate their public \
and private keys.

#### Key sharing

At the start, the client and the server have no knowledge of the \
keys of the other. We will start with a plain TCP connection and then \
pass keys in this order:

```
Client                    Server
   |     Connect TCP        |
  [ ]       ----->          |
   |                        |
   |          Ack           |
   |        <-----         [ ] Standard TCP Connection
   |                        |
   |  Send Client Public    |
  [ ]       ----->          |
   |                       [ ] Start Encryption on server
   |                        |
   |  Send Server Public    |
   |        <-----         [ ]
   |                        |
  [ ]Start Client Encryption|
   |                        |
   |     Ack Encrypted      |
  [ ]       ----->          |
   |                        |
   |     Ack Encrypted      |
  [ ]       <-----          |
   |                        |
```

By doing this handshake, we finish with the knowledge that both \
 client and server can encrypt messaged correctly for each other. \
I am considering the server sending it's public down first such \
 that the client does not reveal any information non encrypted.
