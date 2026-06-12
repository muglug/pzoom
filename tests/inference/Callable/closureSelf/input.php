<?php
class A
{
    /**
     * @var self[]
     */
    private $subitems;

    /**
     * @param self[] $in
     */
    public function __construct(array $in = [])
    {
        array_map(function(self $i): self { return $i; }, $in);

        $this->subitems = array_map(
          function(self $i): self {
            return $i;
          },
          $in
        );
    }
}

new A([new A, new A]);
