<?php
class A {
    /**
     * @return class-string<A> $s
     */
    function foo() : string {
        return get_called_class();
    }
}
