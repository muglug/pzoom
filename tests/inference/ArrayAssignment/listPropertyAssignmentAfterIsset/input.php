<?php
class Collection {
    /** @var list<string> */
    private $list = [];

    public function override(int $offset): void {
        if (isset($this->list[$offset])) {
            $this->list[$offset] = "a";
        }
    }
}
