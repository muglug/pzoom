<?php
$foo = new class {
    public function a() {
        new B();
    }
};
