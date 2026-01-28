<?php
interface EntityInterface
{
    public function getId(): string;
}

/**
 * @phpstan-template T of EntityInterface
 */
interface RepositoryInterface
{
    /**
     * @return T|null
     */
public function byId(string $id);
}

final class Foo implements EntityInterface
{
    public function getId(): string
    {
        return "42";
    }
}

/**
 * @phpstan-implements RepositoryInterface<Foo>
 */
final class FooRepository implements RepositoryInterface
{
    /**
     * @var Foo[]
     */
    public array $elements = [];

    public function byId(string $id): ?Foo
    {
        return $this->elements[$id] ?? null;
    }
}
                
