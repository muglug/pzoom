<?php
/**
 * @property-read string|null $p
 * 
 * @psalm-no-seal-properties // For the test
 */
class A {
    public function __get(string $name) {
        return null;
    }
    public function f(): string {
       return $this->q ?? '';
    }
}
