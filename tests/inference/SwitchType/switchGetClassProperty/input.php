<?php
class A {}
class B {}

class C {
    /** @var mixed */
    public $a;

    function foo() : void {
        if (rand(0, 1)) {
            $this->a = new A();
        }

        switch (get_class($this->a)) {
            case B::class:
                echo "here";
        }
    }
}
