<?php
class A {
    /** @var class-string-map<T,T> */
    private array $map;
    /** @param class-string-map<T,T> $map */
    public function __construct(array $map) {
        $this->map = $map;
    }
}
