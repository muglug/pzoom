<?php
class A {}
class B {
    public function __invoke() : string {
        return "hello";
    }
}
/**
 * @param A|B $c
 */
function foo($c) : void {
    if (is_callable($c)) {
        $c();
    }
}
