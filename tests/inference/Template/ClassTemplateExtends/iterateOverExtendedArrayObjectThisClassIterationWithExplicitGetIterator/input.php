<?php
class O {}
class Foo extends O {
    /** @return Collection<self> */
    public static function getSelfCollection() : Collection {
        return new Collection(self::class);
    }

    public function bar() : void {}
}

/**
 * @template T as O
 * @template-extends ArrayObject<int, T>
 */
class Collection extends ArrayObject
{
    /** @var class-string<T> */
    public $class;

    /** @param class-string<T> $class */
    public function __construct(string $class) {
        $this->class = $class;
    }

    /**
     * @return \ArrayIterator<int, T>
     */
    public function getIterator() {
        return parent::getIterator();
    }
}

/** @return Collection<Foo> */
function getFooCollection() : Collection {
    return new Collection(Foo::class);
}

foreach (getFooCollection() as $i => $foo) {
    $foo->bar();
}

foreach (Foo::getSelfCollection() as $i => $foo) {
    $foo->bar();
}