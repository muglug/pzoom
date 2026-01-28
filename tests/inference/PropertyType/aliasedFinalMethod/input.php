<?php
trait A {
    private int $prop;
    public final function setProp(int $prop): void {
        $this->prop = $prop;
    }
}

class B {
    use A {
        setProp as setPropFinal;
    }

    public function __construct() {
        $this->setPropFinal(1);
    }
}
