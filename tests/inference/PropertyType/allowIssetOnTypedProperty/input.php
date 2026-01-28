<?php
class A {
    public string $a;

    public function __construct(bool $b) {
        if ($b) {
            $this->a = "hello";
        }

        if (isset($this->a)) {
            echo $this->a;
            $this->a = "bello";
        }

        $this->a = "bar";
    }
}
