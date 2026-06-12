<?php
class Vendor
{
    /**
     * @var array
     */
    public array $config = [];
    public function getConfig(): array {return $this->config;}
}

class A extends Vendor
{
    /**
     * @var string[]
     * @psalm-suppress NonInvariantDocblockPropertyType
     */
    public array $config = [];
}
$a = new Vendor();
$_b = new A();
echo (string)($a->getConfig()[0]??"");
