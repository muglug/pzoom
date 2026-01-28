<?php
class O {}
class Foo extends O {
    public function bar() : void {}

    /** @return Collection<self> */
    public static function getSelfCollection() : Collection {
        return new Collection(self::class);
    }
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

foreach (Foo::getSelfCollection() as $i => $foo) {
    $foo->bar();
}