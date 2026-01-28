<?php
abstract class Id
{
    protected string $id;

    final protected function __construct(string $id)
    {
        $this->id = $id;
    }

    /**
     * @return static
     */
    final public static function fromString(string $id): self
    {
        return new static($id);
    }
}

/**
 * @template T of Id
 */
final class Ids
{
    /**
     * @psalm-var list<T>
     */
    private array $ids;

    /**
     * @psalm-param list<T> $ids
     */
    private function __construct(array $ids)
    {
        $this->ids = $ids;
    }

    /**
     * @template T1 of Id
     * @psalm-param T1 $class
     * @psalm-param list<string> $ids
     * @psalm-return self<T1>
     */
    public static function fromObjects(Id $class, array $ids): self
    {
        return new self(array_map([$class, "fromString"], $ids));
    }

    /**
     * @template T1 of Id
     * @psalm-param class-string<T1> $class
     * @psalm-param list<string> $ids
     * @psalm-return self<T1>
     */
    public static function fromStrings(string $class, array $ids): self
    {
        return new self(array_map([$class, "fromString"], $ids));
    }
}