<?php
class A
{
    /** @param array<int, int>|int $className */
    public function a(array|int $className): int
    {
        return 0;
    }
}

class B extends A
{
    /** @param array<int, int>|int|bool $className */
    public function a(array|int|bool $className): int
    {
        return 0;
    }
}

print_r((new A)->a(1));
print_r((new B)->a(true));
                    
