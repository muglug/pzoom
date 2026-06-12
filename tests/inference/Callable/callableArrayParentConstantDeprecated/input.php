<?php
class Z {
    public static function hello(): void {
        echo "hello";
    }
}

class A extends Z {
    public function __construct() {
        $this->run(["parent", "hello"]);
    }

    /**
     * @param callable $callable
     * @return void
     */
    public function run($callable) {
        call_user_func($callable);
    }
}
