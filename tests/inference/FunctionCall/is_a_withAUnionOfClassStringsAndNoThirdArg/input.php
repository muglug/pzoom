<?php
is_a(rand(0, 1) ? InvalidArgumentException::class : RuntimeException::class, Exception::class);
                
