<?php
/**
 * @property-read string|null $p
 */
class A {
    public function __get(string $name) {
        return null;
    }
    public function f(): string {
       return $this->p ?? '';
    }
}
