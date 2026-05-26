use crate::core::Error;

pub(crate) fn set_reuse_port(socket: &socket2::Socket) -> Result<(), Error> {
    socket.set_reuse_port(true).map_err(Error::from)
}
