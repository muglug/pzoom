<?php
class A {
    /** @var class-string-map<T,T> */
    private array $map = [];
    /** @param class-string-map<T,T> $map */
    public function set(array $map): void {
        $this->map = $map;
    }
}
