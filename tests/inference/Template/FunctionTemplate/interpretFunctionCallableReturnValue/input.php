<?php
final class Id
{
    /**
     * @var string
     */
    private $id;

    private function __construct(string $id)
    {
        $this->id = $id;
    }

    public static function fromString(string $id): self
    {
        return new self($id);
    }
}

/**
 * @template T
 * @psalm-param callable(string): T $generator
 * @psalm-return callable(): T
 */
function idGenerator(callable $generator)
{
    return static function () use ($generator) {
        return $generator("random id");
    };
}

function client(Id $id): void
{
}

$staticIdGenerator = idGenerator([Id::class, "fromString"]);
client($staticIdGenerator());