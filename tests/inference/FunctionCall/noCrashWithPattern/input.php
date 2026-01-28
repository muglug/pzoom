<?php
echo !\is_callable($loop_callback)
    || (\is_array($loop_callback)
        && !\method_exists(...$loop_callback));
