<?php
class B {
    /**
     * @return A
     * @psalm-suppress UndefinedDocblockClass
     * @psalm-suppress InvalidReturnStatement
     * @psalm-suppress InvalidReturnType
     */
    public function foo() {
        return new stdClass();
    }

    public function bar() {
        $this->foo()->bar();
    }
}
                    
