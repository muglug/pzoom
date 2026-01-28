<?php
class A {
    public function __invoke(string $p): void {}
}
(new A)->__invoke(1);
