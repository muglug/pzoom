<?php

final class MyVisitor extends ExternalVisitorBase
{
    private int $visitCount = 0;

    public function enterNode(object $node): ?int
    {
        $this->visitCount++;

        return $this->describe($node);
    }

    private function describe(object $node): ?int
    {
        return null;
    }
}

new MyVisitor();
