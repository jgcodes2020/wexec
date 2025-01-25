# wexec
A batteries-included async executor built on `winit`. Just as `winit` is derived from "window 
initializer", `wexec` comes from "window executor".

## Why?
Winit's event-loop model lends itself to using a state machine to control the application lifecycle. These can be tedious to write.

`async` allows us to replace clunky state machines with clean, imperative-like code, and as a bonus, allows the use of lifetime-bound references for app state!

## Compatibility
`wexec` should be directly compatible with `async_std` since it uses an off-thread reactor for I/O;
however, using Tokio will require spinning up a separate runtime.