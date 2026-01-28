<?php
/** @template T1 of object */
abstract class ParentClass {
    /** @var class-string<T1> */
    protected $c;

    /** @param class-string<T1> $c */
    public function __construct(string $c) {
        $this->c = $c;
    }

    /** @return class-string<T1> */
    abstract public function foo(): string;
}

/**
 * @template T2 of object
 * @extends ParentClass<T2>
 */
class ChildClass extends ParentClass {
    public function foo(): string {
        return $this->c;
    }
}