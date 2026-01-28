<?php
class A {
    /** @var int */
    public $a = 1;
    /** @var int */
    public $b = 2;

    /** @return properties-of<static> */
    public function asArray() {
        return [
            "a" => $this->a,
            "b" => $this->b
        ];
    }
}

class B extends A {
    /** @var int */
    public $c = 3;
}

class C extends B {
    /** @var int */
    public $d = 4;

    public function asArray() {
        return [
            "a" => $this->a,
            "b" => $this->b,
            "c" => $this->c,
            "d" => $this->d,
        ];
    }
}
                
