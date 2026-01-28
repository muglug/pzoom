<?php
/**
 * @template T
 * @param callable(T):void $c
 * @param T $param
 */
function apply(callable $c, $param):void{
    call_user_func($c, $param);
}

class A {
    public function __toString(){
        return "a";
    }
}

class B {}

class Printer{
    public function __invoke(A $a) : void {
        echo $a;
    }
}

apply(new Printer(), new B());
