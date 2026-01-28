<?php
class A {
    /** @var string|null */
    public $foo;

    public function __get(string $var_name) : ?string {
        if ($var_name === "foo") {
            return $this->$var_name;
        }

        return null;
    }
}
