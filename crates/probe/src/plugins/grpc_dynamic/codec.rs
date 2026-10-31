use prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use tonic::{
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
    Status,
};
pub(super) struct DynamicCodec(pub MessageDescriptor);
pub(super) struct DynamicEncoder;
pub(super) struct DynamicDecoder(MessageDescriptor);
impl Encoder for DynamicEncoder {
    type Item = DynamicMessage;
    type Error = Status;
    fn encode(
        &mut self,
        item: DynamicMessage,
        destination: &mut EncodeBuf<'_>,
    ) -> Result<(), Status> {
        item.encode(destination)
            .map_err(|_| Status::invalid_argument("cannot encode protobuf message"))
    }
}
impl Decoder for DynamicDecoder {
    type Item = DynamicMessage;
    type Error = Status;
    fn decode(&mut self, source: &mut DecodeBuf<'_>) -> Result<Option<DynamicMessage>, Status> {
        DynamicMessage::decode(self.0.clone(), source)
            .map(Some)
            .map_err(|_| Status::internal("invalid protobuf response"))
    }
}
impl Codec for DynamicCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynamicEncoder;
    type Decoder = DynamicDecoder;
    fn encoder(&mut self) -> DynamicEncoder {
        DynamicEncoder
    }
    fn decoder(&mut self) -> DynamicDecoder {
        DynamicDecoder(self.0.clone())
    }
}
