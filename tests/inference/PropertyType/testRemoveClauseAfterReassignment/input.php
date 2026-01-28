<?php
class Test {
    /** @var ?bool */
    private $foo;

    public function run(): void {
        $this->foo = false;
        $this->bar();
        if ($this->foo === true) {}
    }

    private function bar(): void {
        if (mt_rand(0, 1)) {
            $this->foo = true;
        }
    }
}
