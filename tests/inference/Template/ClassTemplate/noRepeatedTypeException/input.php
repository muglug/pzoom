<?php
/** @template T as object */
class Foo
{
    /**
     * @psalm-var class-string<T>
     */
    private $type;

    /** @var array<T> */
    private $items;

    /**
     * @param class-string<T> $type
     */
    public function __construct(string $type)
    {
        if (!in_array($type, [A::class, B::class], true)) {
            throw new \InvalidArgumentException;
        }

        $this->type = $type;
        $this->items = [];
    }

    /** @param T $item */
    public function add($item): void
    {
        $this->items[] = $item;
    }
}

class FooFacade
{
    /**
     * @template T as object
     * @param  T $item
     */
    public function add(object $item): void
    {
        $foo = $this->ensureFoo([$item]);
        $foo->add($item);
    }

    /**
     * @template T as object
     * @param  array<mixed,T> $items
     * @return Foo<T>
     */
    private function ensureFoo(array $items): Foo
    {
        /** @var class-string<T> */
        $type = $items[0] instanceof A ? A::class : B::class;
        return new Foo($type);
    }
}

class A {}
class B {}