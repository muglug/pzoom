<?php
class A {
    public function __construct() {
        $this->run("self::hello");
    }

    public static function hello(): void {
        echo "hello";
    }

    /**
     * @param callable $callable
     * @return void
     */
    public function run($callable) {
        call_user_func($callable);
    }
}
