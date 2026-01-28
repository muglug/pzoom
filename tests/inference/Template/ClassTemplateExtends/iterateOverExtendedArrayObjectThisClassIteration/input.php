<?php
class O {}

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

    private function iterate() : void {
        foreach ($this as $o) {}
    }
}