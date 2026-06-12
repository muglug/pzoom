<?php
function may_exit() : void {
    exit(0);
}

class InClass {
    /**
     * @psalm-suppress InvalidReturnType
     * @psalm-return never-returns
     */
    function test() {
        may_exit();
    }
}
