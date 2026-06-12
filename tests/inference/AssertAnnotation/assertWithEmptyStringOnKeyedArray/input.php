<?php
class A
{
    function test(): void
    {
        $a = ["" => ""];

        /** @var array<string, mixed> $b */
        $b = [];

        $this->assertSame($a, $b);
    }

    /**
     * @template T
     * @param T      $expected
     * @param mixed $actual
     * @psalm-assert =T $actual
     */
    public function assertSame($expected, $actual): void
    {
        return;
    }
}
