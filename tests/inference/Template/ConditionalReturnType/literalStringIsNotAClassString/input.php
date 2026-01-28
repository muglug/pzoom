<?php
interface SerializerInterface
{
    /**
     * Deserializes the given data to the specified type.
     *
     * @psalm-template TClass
     * @psalm-template TType as 'array'|class-string<TClass>
     * @psalm-param TType $type
     * @psalm-return (
     *     TType is 'array'
     *     ? array
     *     : TClass
     * )
     *
     * @return mixed
     */
    public function deserialize(string $data, string $type);
}

function foo(SerializerInterface $i, string $data): Exception {
    return $i->deserialize($data, Exception::class);
}

function bar(SerializerInterface $i, string $data): array {
    return $i->deserialize($data, 'array');
}