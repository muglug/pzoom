<?php
class C {
    /** @var int */
    private $id = 42;

    /** @return non-empty-list<int> */
    public function f(): array {
        return array_column([new self], "id");
    }
}
            
