<?php
class A {}

class B extends A {
    /**
     * @return class-string<A> $s
     */
    function foo() : string {
        return get_parent_class();
    }
}
