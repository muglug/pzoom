<?php
class A {
    public function __invoke(): bool { return true; }
}

class C {
    /** @var A $invokable */
    private $invokable;

    public function __construct(A $invokable) {
        $this->invokable = $invokable;
    }

    public function callTheInvokableDirectly(): bool {
        return ($this->invokable)();
    }

    public function callTheInvokableIndirectly(): bool {
        $r = $this->invokable;
        return $r();
    }
}
