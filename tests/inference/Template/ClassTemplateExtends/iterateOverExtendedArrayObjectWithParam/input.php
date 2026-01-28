<?php
class O {}
class Foo extends O {
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
}

/** @return Collection<Foo> */
function getFooCollection() : Collection {
    return new Collection(Foo::class);
}

foreach (getFooCollection() as $i => $foo) {
    $foo->bar();
}