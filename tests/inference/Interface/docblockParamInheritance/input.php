<?php
interface I {
    /** @param string[] $f */
    function foo(array $f) : void {}
}

class C implements I {
    /** @var string[] */
    private $f = [];

    /**
     * {@inheritdoc}
     */
    public function foo(array $f) : void {
        $this->f = $f;
    }
}

class C2 implements I {
    /** @var string[] */
    private $f = [];

    /**
     * {@inheritDoc}
     */
    public function foo(array $f) : void {
        $this->f = $f;
    }
}
