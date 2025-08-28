So, kind of the main gist here is that we want to build up a protocol for what messages are and what the should do.

If we us length delineated codec from tokio, we can just get the raw bytes that are send per command.
If we send and receive raw rust structs then we can play with the raw bytes a bit.

We will split commands from clients into 2 sections:
Non Authenticated
Authenticated

Non Authenticated commands will be used For the sake of authentication and server status.

Authentication:
To just build a safety non necessary auth system.
When a user registers, we can increment a counter to decide their id, not bothering to persist user data at this point.

We'll make this a u32 and assign it as the user's public id.
When we register this internally, we will store it in a tuple with the address of the connection.

Doing this, we can assert that messages as a user must come from a specific socket addr.
While this is not exactly secure, nothing is saved so it really doesn't matter.
