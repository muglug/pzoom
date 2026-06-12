<?php
class ReaderC {}
class BaseC {
    /**
     * @param object $target
     * @param string $delimiter
     */
    public function __construct($target, $delimiter = '->') {}
}

class ChildC extends BaseC {
    public function __construct(
        protected ReaderC $reader,
        protected int $count,
    ) {
        parent::__construct($this, '/');
        takesReader($reader);
        takesInt($count);
    }
}

function takesReader(ReaderC $r): void {}
function takesInt(int $i): void {}
