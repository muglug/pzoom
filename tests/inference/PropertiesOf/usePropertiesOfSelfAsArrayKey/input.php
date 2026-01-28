<?php
class A {
    /** @var int */
    public $a = 1;
    /** @var int */
    public $b = 2;

    /** @return properties-of<self> */
    public function asArray() {
        return [
            "a" => $this->a,
            "b" => $this->b
        ];
    }
}
                
