<?php
class B {
    /**
     * @psalm-param callable(mixed...):static $i
     */
    function takesACall(callable $i) : void {}

    public function call() : void {
        $this->takesACall(function() {return $this;});
    }
}
