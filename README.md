A cli for interacting with the data from https://vaccinespotter.org. This tool will
hit their api for your state once a minute and either print the results to the terminal
or send an email with the new appointments

```
vaccine_spotter 0.1.0

USAGE:
    vaccine_spotter [OPTIONS] --state <state>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -f, --from-email <from-email>    The email address to send alerts from
    -s, --state <state>              the 2 digit state code to use to get current appointments
    -t, --to-email <to-email>        The email address to send alerts to
    -z, --zips-path <zips-path>      The path to a json file containing an array of strings representing the target zip
                                     codes. If not provided all zipcodes will be considered
```

If either emails are omitted from the options, it will simply print to stdout

