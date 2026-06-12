<?php
class ParentClass {
    private string $prop;

    public function __construct() {
        $this->init();
    }

    public function init(): void {
        $this->prop = "zxc";
    }
}

class ChildClass extends ParentClass {
    public function init(): void {}
}
