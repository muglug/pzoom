<?php
/**
 * @template-covariant T
 */
class A {
    private $arr;

    /** @psalm-param array<T> $arr */
    public function __construct(array $arr) {
        $this->arr = $arr;
    }

    /** @psalm-return array<T> */
    public function foo(): array {
        return $this->arr;
    }
}