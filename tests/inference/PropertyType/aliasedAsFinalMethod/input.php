<?php
trait A {
    private int $prop;
    public function setProp(int $prop): void {
        $this->prop = $prop;
    }
}

class B {
    use A {
        setProp as final setPropFinal;
    }

    public function __construct() {
        $this->setPropFinal(1);
    }
}
