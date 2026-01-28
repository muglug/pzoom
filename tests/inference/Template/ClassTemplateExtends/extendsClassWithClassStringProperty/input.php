<?php
class Some {}

/** @template T of object */
abstract class Y {
    /** @var class-string<T> */
    protected $c;
}

/**
 * @template T of Some
 * @extends Y<Some>
 */
class Z extends Y {
    /** @param class-string<T> $c */
    public function __construct(string $c) {
        $this->c = $c;
    }
}