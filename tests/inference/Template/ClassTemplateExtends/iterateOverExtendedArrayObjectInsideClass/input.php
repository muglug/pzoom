<?php
class O {}
class Foo extends O {
    public function bar() : void {}

    /**
     * @param Collection<self> $c
     */
    public static function takesSelfCollection(Collection $c) : void {
        foreach ($c as $i => $foo) {
            $foo->bar();
        }
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