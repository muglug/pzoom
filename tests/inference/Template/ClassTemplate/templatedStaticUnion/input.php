<?php
/**
 * @template T
 * @psalm-consistent-templates
 */
abstract class A {
    /**
      * @var T
      */
    private $v;

    /**
      * @param T $v
      */
    final public function __construct($v) {
        $this->v = $v;
    }

    /**
      * @return static<T>
      */
    public function foo(): A {
        if (rand(0, 1)) {
            return new static($this->v);
        } else {
            return new static($this->v);
        }
    }
}