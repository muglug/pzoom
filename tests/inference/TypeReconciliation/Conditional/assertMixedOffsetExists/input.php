<?php
class A {
    /** @var mixed */
    private $arr;

    /**
     * @psalm-suppress MixedArrayAccess
     * @psalm-suppress MixedReturnStatement
     * @psalm-suppress MixedArrayAssignment
     */
    public function foo() : stdClass {
        if (isset($this->arr[0])) {
            return $this->arr[0];
        }

        $this->arr[0] = new stdClass;
        return $this->arr[0];
    }
}
