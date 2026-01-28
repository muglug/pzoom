<?php
class Foo {}

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
     * @return class-string<T>|null
     */
    public function getType()
    {
       return $this->type;
    }
}

function foo(Collection $c) : void {
    $val = $c->getType();
    if (!$val) {}
    if ($val) {}
}