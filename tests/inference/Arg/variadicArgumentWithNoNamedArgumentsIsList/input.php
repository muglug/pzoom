<?php
class A {
    /**
     * @no-named-arguments
     * @psalm-return list<int>
     */
    public function foo(int ...$values): array
    {
        return $values;
    }
}
                
