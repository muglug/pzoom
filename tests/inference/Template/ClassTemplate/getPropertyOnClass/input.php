<?php
class Foo {
    /** @var int */
    public $id = 0;
}

/**
 * @template T as Foo
 */
class Collection {
    /**
     * @var class-string<T>
     */
    private $type;

    /**
     * @param class-string<T> $type
     */
    public function __construct(string $type) {
        $this->type = $type;
    }

    /**
     * @return class-string<T>
     */
    public function getType()
    {
       return $this->type;
    }

    /**
     * @param T $object
     */
    public function bar(Foo $object) : void
    {
        if ($this->getType() !== get_class($object)) {
            return;
        }

        echo $object->id;
    }
}

class FooChild extends Foo {}

/** @param Collection<Foo> $c */
function handleCollectionOfFoo(Collection $c) : void {
    if ($c->getType() === FooChild::class) {}
}