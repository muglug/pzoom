<?php
class A {
    public function foo() : array {
        return [1, 2, 3];
    }
}

(new A)->foo()[3] = 5;
