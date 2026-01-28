<?php
class C {
    private ?\Closure $onCancel;

    public function __construct() {
        $this->foo($this->onCancel);
    }

    /**
     * @param mixed $onCancel
     * @param-out \Closure $onCancel
     */
    public function foo(&$onCancel) : void {
        $onCancel = function (): void {};
    }
}
