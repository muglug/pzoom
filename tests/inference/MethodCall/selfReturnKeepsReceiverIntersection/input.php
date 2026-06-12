<?php

interface ExpectationLike {
    /** @return self */
    public function andReturn(string $value);
}

interface MockLike {
    /** @return ExpectationLike&static */
    public function shouldReceive(string $name);
}

function buildChain(MockLike $mock): void {
    $chain = $mock->shouldReceive('first')
        ->andReturn('a')
        ->shouldReceive('second')
        ->andReturn('b');
    echo get_class($chain);
}
