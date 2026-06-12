<?php
class Foo {
    public function __construct() {
        $this->run_in_c(array($this, "hello"));
    }

    public function hello(): void {
        echo "hello";
    }

    /**
     * @param callable $callable
     * @return void
     */
    public function run_in_c($callable) {
        call_user_func($callable);
    }
}
