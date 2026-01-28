<?php
class a
{
    public function fluent(): self
    {
        return $this;
    }
}

$a = new a();
if (!$a->fluent()) {
    echo "always";
}
                    
