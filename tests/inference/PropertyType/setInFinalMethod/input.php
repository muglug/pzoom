<?php
class C
{
    /**
     * @var string
     */
    private $a;

    /**
     * @var string
     */
    private $b;

    /**
     * @param string[] $opts
     * @psalm-param array{a:string,b:string} $opts
     */
    public function __construct(array $opts)
    {
        $this->setOptions($opts);
    }

    /**
     * @param string[] $opts
     * @psalm-param array{a:string,b:string} $opts
     */
    final public function setOptions(array $opts): void
    {
        $this->a = $opts["a"] ?? "defaultA";
        $this->b = $opts["b"] ?? "defaultB";
    }
}
