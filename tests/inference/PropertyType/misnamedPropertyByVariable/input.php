<?php
class B {
    /** @var string|null */
    public $foo;

    public function bar(string $var_name) : ?string {
        if ($var_name === "bar") {
            return $this->$var_name;
        }

        return null;
    }
}
