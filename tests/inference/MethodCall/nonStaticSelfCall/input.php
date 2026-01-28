<?php
class A11 {
    public function call() : self {
        $result = self::method();
        return $result;
    }

    public function method() : self {
        return $this;
    }
}
$x = new A11();
var_export($x->call());
