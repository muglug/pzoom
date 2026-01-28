<?php
class Base {
    /**
     * @template T
     * @param T $x
     * @return T
     */
    function example($x) {
        return $x;
    }
}

class Child extends Base {
    function example($x) {
        return $x;
    }
}

ord((new Child())->example("str"));