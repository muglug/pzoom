<?php
interface I {}
class C implements I {
    public function foo() : self {
        bar($this);
        return $this;
    }
}

function bar(I &$i) : void {}
