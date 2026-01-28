<?php
class D {
    private string $s;

    public function __construct(string $s) {
        $this->s = $s;
    }

    /**
     * @psalm-mutation-free
     */
    public function getShort() : string {
        return substr($this->s, 0, 5);
    }

    /**
     * @psalm-mutation-free
     */
    public function getShortMutating() : string {
        $this->s = "hello";
        return substr($this->s, 0, 5);
    }
}
