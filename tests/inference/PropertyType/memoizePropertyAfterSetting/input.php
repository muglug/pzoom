<?php
class A {
    public function foo() : void {
        /** @psalm-suppress UndefinedThisPropertyAssignment */
        $this->b = "c";
        echo strlen($this->b);
    }
}
