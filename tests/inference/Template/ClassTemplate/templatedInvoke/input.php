<?php
/**
 * @template T
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class Foo {
    /** @var T */
    private $value;

    /** @param T $val */
    public function __construct($val) {
        $this->value = $val;
    }

    /** @return T */
    public function get() {
        return $this->value;
    }

    /**
     * @param T $val
     * @return Foo<T>
     */
    public function __invoke($val) {
        return new static($val);
    }

    /**
     * @param T $val
     * @return Foo<T>
     */
    public function create($val) {
        return new static($val);
    }
}

function bar(string $s) : string {
    $foo = new Foo($s);
    $bar = $foo($s);
    return $bar->get();
}