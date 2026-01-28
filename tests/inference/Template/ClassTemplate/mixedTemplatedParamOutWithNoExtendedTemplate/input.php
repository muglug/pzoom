<?php
/**
 * @template TValue
 */
class ValueContainer
{
    /**
     * @var TValue
     */
    private $v;
    /**
     * @param TValue $v
     */
    public function __construct($v)
    {
        $this->v = $v;
    }
    /**
     * @return TValue
     */
    public function getValue()
    {
        return $this->v;
    }
}

/**
 * @psalm-suppress MissingTemplateParam
 * @template TKey
 * @template TValue
 */
class KeyValueContainer extends ValueContainer
{
    /**
     * @var TKey
     */
    private $k;
    /**
     * @param TKey $k
     * @param TValue $v
     */
    public function __construct($k, $v)
    {
        $this->k = $k;
        parent::__construct($v);
    }
    /**
     * @return TKey
     */
    public function getKey()
    {
        return $this->k;
    }
}
$a = new KeyValueContainer("hello", 15);
$b = $a->getValue();
