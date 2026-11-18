# {{name}}

A PHP application for the [Phera](https://github.com/GijsRommers/Phera) runtime.

## Run it

```sh
phera serve public
```

Then open <http://127.0.0.1:8080/> or <http://127.0.0.1:8080/hello?name=you>.

## Where things are

| Path | What it is |
| --- | --- |
| `public/index.php` | The front controller. Every request that is not a file in `public/` goes here. |
| `public/` | Files the server sends as they are: images, CSS, JavaScript. |
| `src/Application.php` | The routes of the application. |
| `src/Http/` | The request and response objects. |
| `composer.json` | The packages of the project, and the rule that finds classes: `App\Name` is `src/Name.php`. |
| `.env` | Settings, read when the application starts. |

## Check it

```sh
phera check src public
```

reports type mistakes without running the code.
