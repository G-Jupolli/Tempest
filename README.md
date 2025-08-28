# Tempest Games

This is my attempt at a game server implementation.

I am also going to include some exploratory attempts at front ends:

-   Cli Interface ( Ratatui )
-   Desktop app ( Gpui )
-   Web ( Leptos )

The goal here is to have a pure rust implementation across these platform to serve \
as an example for using these technologies in a more in depth example.

The server is also Buildable for self hosting ( Need to make the DockerFile for this ).

### Current State:

I have the server working for most message passing and a ( almost finished ) implementation of \
Uno on the serve and the CLI.

### Next Steps:

-   Finish Uno implementation on Cli & Server
-   Cleanup code and refactor some bad messages
-   Better documentation on CLI & Server
-   Server Diagrams in figma

### For the Future:

The CLI and Server are my main focus for now, The ideal is to fully finish them and then \
try to get some feedback on the project as a whole.

A Web interface would be nice but web is what I do most of the time and would like something else. \
While Gpui is still actively being worked on, I'd like to try it and possible add some commits if I am \
able to as I go, native applications are something I should learn more about.

I'm trying to build the systems & server such that there can be other games too. \
While this would be nice, I care more about having a fully working system before I do \
something like that.
