<?php
/**
 * @psalm-immutable
 */
final class ClientId
{
    public string $id;

    private function __construct(string $id)
    {
        $this->id = $id;
    }

    public static function fromString(string $id): self
    {
        return new self($id . rand(0, 1));
    }
}
