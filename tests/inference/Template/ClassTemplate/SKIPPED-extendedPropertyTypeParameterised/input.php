<?php
namespace App;

use DateTimeImmutable;
use Ds\Map;

abstract class Z
{
    public function test(): void
    {
        $map = $this->createMap();

        $date = $map->get("test");

        echo $date->format("Y");
    }

    /**
     * @return Map<string, DateTimeImmutable>
     */
    abstract protected function createMap(): Map;
}