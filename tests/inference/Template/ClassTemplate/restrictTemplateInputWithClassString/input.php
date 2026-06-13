<?php
/** @template T as object */
class Foo
{
    /**
     * @psalm-var class-string
     */
    private $type;

    /** @var array<T> */
    private $items;

    /**
     * @param T::class $type
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

class A {}
class B {}

$foo = new Foo(A::class);
$foo->add(new B);
