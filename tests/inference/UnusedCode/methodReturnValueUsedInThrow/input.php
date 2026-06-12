<?php
final class A {
    public function foo() : Exception {
        return new Exception;
    }
}
throw (new A)->foo();

