var view = undefined;
var current = undefined;
var viewing = undefined;
var invocation = {};
var hosts = {};
var invocations = {};
var hostStates = {};
var snackbar = [];

let View = class {
  constructor() {
    this.name = document.getElementById("viewing_name");
    this.start = document.getElementById("viewing_time");
    this.id = document.getElementById("viewing_id");
    this.url = document.getElementById("viewing_url");
    this.hash = document.getElementById("viewing_hash");
    this.reinvoke = document.getElementById("reinvoke");
    this.cancel = document.getElementById("cancel");
    this.setup = document.getElementById("setup");
    this.genLogs = document.getElementById("viewing_log_gen_note");
    this.logDir = document.getElementById("viewing_log_dir");
    this.content = document.getElementById("content");
    this.placeholder = document.getElementById("center_placeholder");
    this.reinvokeEvent = undefined;
    this.cancelEvent = undefined;
  }

  hide() {
    this.placeholder.classList.remove("hidden");
    this.content.classList.add("hidden");
  }

  render() {
    if (invocation !== undefined) {
      hostStates = {};
      this.placeholder.classList.add("hidden");
      this.content.classList.remove("hidden");
      empty(this.name);
      this.name.appendChild(document.createTextNode(invocation.descriptor.name));
      empty(this.start);
      var time = document.createTextNode(formatDate(new Date(invocation.start)));
      this.start.appendChild(time);
      empty(this.id);
      this.id.appendChild(document.createTextNode(invocation.id));
      empty(this.url);
      this.url.setAttribute("href", invocation.url);
      this.url.appendChild(document.createTextNode(invocation.url));
      empty(this.hash);
      this.hash.appendChild(document.createTextNode(invocation.commit));
      if (this.reinvokeEvent !== undefined) {
        this.reinvoke.removeEventListener("click", this.reinvokeEvent);
      }
      this.reinvokeEvent = function() {
        displaySnackbar("attempting to reclone repository");
        get("/api/reinvoke/" + invocation.id, function(response) {
          updateCurrent();
          viewing = response.id;
          invocation = response;
          view.render();
        }, function(err) {
          displaySnackbar(err);
        })
      };
      this.reinvoke.addEventListener("click", this.reinvokeEvent);
      if (this.cancelEvent !== undefined) {
        this.cancel.removeEventListener("click", this.cancelEvent);
      }
      if (invocation.id === current) {
        this.cancel.classList.remove("hidden");
        this.cancelEvent = function() {
          displaySnackbar("attempting to cancel invocation");
          get("/api/cancel", function() {
            updateCurrent();
            document.getElementById("cancel").classList.add("hidden");
          }, function(err) {
            displaySnackbar(err);
          });
        };
        this.cancel.addEventListener("click", this.cancelEvent);
      } else {
        this.cancel.classList.add("hidden");
      }
      if (invocation.descriptor.gen_logs) {
        this.genLogs.classList.remove("hidden");
      } else {
        this.genLogs.classList.add("hidden");
      }
      empty(this.logDir);
      this.logDir.appendChild(document.createTextNode(invocation.descriptor.log_dir));
      empty(this.setup);
      if (invocation.descriptor.command !== null) {
        var global = document.createElement("h3");
        global.appendChild(document.createTextNode("global setup"));
        this.setup.appendChild(global);
        this.setup.appendChild(makeCommand(invocation.descriptor.command, invocation.descriptor.args));
      }
      var hostHeader = document.createElement("h3");
      hostHeader.appendChild(document.createTextNode("hosts"));
      this.setup.appendChild(hostHeader);
      for (var host in invocation.descriptor.hosts) {
        var hostname = document.createElement("p");
        hostname.classList.add("hostname");
        hostname.appendChild(document.createTextNode(host));
        var state = document.createElement("a");
        hostStates[host] = state;
        updateHostState(host);
        hostname.appendChild(state);
        this.setup.appendChild(hostname);
        var record = invocation.descriptor.hosts[host];
        if (record.command !== null) {
          this.setup.appendChild(makeCommand(record.command, record.args));
        }
      }
    }
  }
}

let Host = class {
  constructor(record) {
    this.id = record.id;
    this.hostname = record.hostname;
    this.state = record.state.desc;
    this.target = record.state.id;
    this.listing = undefined;
  }

  makeId() {
    var element = document.createElement("div");
    element.classList.add("id");
    element.appendChild(document.createTextNode(this.id));
    return element;
  }

  makeName() {
    var element = document.createElement("div");
    element.classList.add("hostname");
    element.appendChild(document.createTextNode(this.hostname));
    return element;
  }

  makeState() {
    var element = document.createElement("a");
    element.classList.add("state");
    element.classList.add(this.state);
    element.appendChild(document.createTextNode(this.state));
    if (this.target !== null) {
      var target = this.target;
      element.addEventListener("click", function() {
        viewInvocation(target);
      });
    }
    return element;
  }

  get element() {
    if (this.listing === undefined) {
      this.listing = document.createElement("div");
      this.listing.classList.add("host");
      this.listing.appendChild(this.makeId());
      this.listing.appendChild(this.makeName());
      this.listing.appendChild(this.makeState());
      this.listing.setAttribute("uuid", this.id);
    }
    return this.listing;
  }
};

let Invocation = class {
  constructor(record) {
    this.id = record.id;
    this.name = record.name;
    this.url = record.url;
    this.commit = record.commit;
    this.start = record.start;
    this.listing = undefined;
    if (this.name === null) {
      this.failed = true;
    } else {
      this.failed = false;
    }
  }

  makeId() {
    var element = document.createElement("div");
    element.classList.add("id");
    element.appendChild(document.createTextNode(this.id));
    return element;
  }

  makeName() {
    var element = document.createElement("div");
    element.classList.add("name");
    if (this.failed) {
      element.classList.add("unresolved");
      element.appendChild(document.createTextNode("(failed)")); 
    } else {
      element.appendChild(document.createTextNode(this.name));
    }
    return element;
  }

  makeUrl() {
    var element = document.createElement("a");
    element.classList.add("url");
    element.setAttribute("href", this.url);
    element.appendChild(document.createTextNode(this.url));
    return element;
  }

  makeCommit() {
    var element = document.createElement("div");
    element.classList.add("commit");
    element.appendChild(document.createTextNode(this.commit.substring(0, 10)));
    return element;
  }

  makeExpandButton() {
    var element = document.createElement("a");
    element.classList.add("popout");
    element.classList.add("button");
    element.appendChild(materialIcon("open_in_new"));
    var id = this.id;
    element.addEventListener("click", function() {
      viewInvocation(id);
    });
    return element;
  }

  makeTime() {
    var element = document.createElement("div");
    element.classList.add("time");
    element.appendChild(document.createTextNode(formatDate(new Date(this.start))));
    return element;
  }

  makeStatus() {
    var element;
    if (this.failed) {
      element = materialIcon("clear");
    } else {
      element = materialIcon("check");
      element.classList.add("ok");
    }
    element.classList.add("status");
    return element;
  }

  get element() {
    if (this.listing === undefined) {
      this.listing = document.createElement("div");
      this.listing.classList.add("invocation");
      this.listing.appendChild(this.makeId());
      this.listing.appendChild(this.makeName());
      this.listing.appendChild(this.makeUrl());
      this.listing.appendChild(this.makeCommit());
      if (!this.failed) {
        this.listing.appendChild(this.makeExpandButton());
      }
      this.listing.appendChild(this.makeTime());
      this.listing.appendChild(this.makeStatus());
      this.listing.setAttribute("uuid", this.id);
    }
    return this.listing;
  }
};

function get(url, callback, err) {
  var xhttp = new XMLHttpRequest();
  xhttp.open("GET", url);
  xhttp.send();
  xhttp.onreadystatechange = (e) => {
    if (xhttp.readyState === 4) {
      var response;
      try {
        response = JSON.parse(xhttp.responseText);
      } catch (e) { return; }
      if (response.status == "ok") {
        delete response.status;
        if ('payload' in response) {
          callback(response.payload);
        } else {
          callback();
        }
      } else {
        if (!('msg' in response)) {
          response.msg = "an error occured";
        }
        err(response.msg);
      }
    }
  }
}

function pad(string) {
  string = "" + string;
  if (string.length == 1) {
    return "0" + string;
  } else {
    return string;
  }
}

function formatDate(date) {
  return pad(date.getHours()) + ":" +
      pad(date.getMinutes()) + ":" +
      pad(date.getSeconds()) + " " +
      date.getDate() + "/" +
      (date.getMonth() + 1) + "/" +
      date.getFullYear();
}

function materialIcon(name) {
  var element = document.createElement("i");
  element.classList.add("material-icons");
  element.appendChild(document.createTextNode(name));
  return element;
}

function makeEmpty() {
  var element = document.createElement("div");
  element.classList.add("invocation");
  var p = document.createElement("p");
  p.classList.add("placeholder");
  p.appendChild(document.createTextNode("no active invocation"));
  element.appendChild(p);
  return element;
}

function empty(element) {
  while (element.firstChild) {
    element.removeChild(element.firstChild);
  }
}

function updateCurrent() {
  var active = document.getElementById("active");
  get("/api/current", function(id) {
    if (current !== id) {
      current = id;
      updateInvocations(function() {
        empty(active);
        active.appendChild(invocations[current].element);
      });
    }
  }, function(err) {
    current = undefined;
    updateInvocations(function() {
      empty(active);
      active.appendChild(makeEmpty());
    });
  });
}

function updateInvocations(callback) {
  get("/api/invocations", function(response) {
    for (record of response) {
      if (!(record.id in invocations)) {
        invocations[record.id] = new Invocation(record);
      }
    }
    for (id in invocations) {
      var index = response.findIndex(function(record) {
        return record.id == id;
      });
      if (index === -1) {
        delete invocations[id];
      }
    }
    var list = document.getElementById("invocations");
    var children = [];
    if (current !== undefined) {
      children.push(current);
    }
    for (child of list.children) {
      var id = child.getAttribute("uuid");
      children.push(id);
      if (id === current || !(id in invocations)) {
        list.removeChild(child);
      }
    }
    for (id in invocations) {
      if (!children.includes(id)) {
        list.appendChild(invocations[id].element);
      }
    }
    var placeholder = document.getElementById("history_placeholder");
    if (list.children.length == 0) {
      placeholder.classList.remove("hidden");
    } else {
      placeholder.classList.add("hidden");
    }
    if (viewing !== undefined && !children.includes(viewing)
        && (current === undefined || current !== viewing)) {
      viewing = undefined;
      view.hide();
    }
    callback();
  }, function(err) {
    displaySnackbar(err);
  });
}

function updateHosts() {
  get("/api/hosts", function(response) {
    var list = document.getElementById("hosts");
    empty(list);
    for (record of response) {
      list.appendChild((new Host(record)).element);
      hosts[record.hostname] = record;
      updateHostState(record.hostname);
    }
    var placeholder = document.getElementById("hosts_placeholder");
    if (list.children.length == 0) {
      placeholder.classList.remove("hidden");
    } else {
      placeholder.classList.add("hidden");
    }
  }, function(err) {
    element.classList.add("show");
  });
}

function updateHostState(host) {
  if (host in hostStates) {
    var element = hostStates[host];
    empty(element);
    element.classList = [];
    element.classList.add("state");
    element.removeAttribute("href");
    if (host in hosts) {
      if (hosts[host].hostname in invocation.logs) {
        element.classList.add("logs");
        element.appendChild(document.createTextNode("logs"));
        element.setAttribute("href", invocation.logs[hosts[host].hostname]);
      } else if (hosts[host].state.id === viewing) {
        element.classList.add(hosts[host].state.desc);
        element.appendChild(document.createTextNode(hosts[host].state.desc));
      } else if (!('id' in hosts[host].state)) {
        element.classList.add(hosts[host].state.desc);
        element.appendChild(document.createTextNode(hosts[host].state.desc));
      } else if (viewing === current) {
        element.classList.add("busy");
        element.appendChild(document.createTextNode("busy"));
      } else {
        element.classList.add("abandoned");
        element.appendChild(document.createTextNode("abandoned"));
      }
    } else {
      element.classList.add("disconnected");
      element.appendChild(document.createTextNode("disconnected"));
    }
  }
}

function makeCommand(command, args) {
  for (arg of args) {
    if (arg.includes(" ")) {
      if (!arg.includes("\"")) {
        arg = "\"" + arg + "\"";
      } else {
        arg = "\'" + arg + "\'";
      }
    }
    command += " " + arg;
  }
  var element = document.createElement("pre");
  element.appendChild(document.createTextNode(command));
  return element;
}

function viewInvocation(id) {
  get("/api/invocation/" + id, function(response) {
    viewing = id;
    invocation = response;
    view.render();
  }, function(err) {
    displaySnackbar(err);
  });
}

function updateSnackbar() {
  var element = document.getElementById("snackbar");
  if (snackbar.length > 0 && !element.classList.contains("show")) {
    empty(element);
    element.appendChild(document.createTextNode(snackbar.shift()));
    setTimeout(function() {
      element.classList.remove("show");
    }, 3250);
    element.classList.add("show");
  }
}

function displaySnackbar(msg) {
  snackbar.push(msg);
}

setInterval(updateCurrent, 500);
setInterval(updateHosts, 500);
setInterval(updateSnackbar, 100);

document.addEventListener('DOMContentLoaded', function() {
  view = new View();
  updateCurrent();
  updateHosts();
  document.getElementById("invoke_button").addEventListener("click", function() {
    displaySnackbar("attempting to clone repository");
    var url = encodeURIComponent(document.getElementById('input').value).trim();
    if (url.length > 0) {
      get("/api/invoke/" + url, function(response) {
        document.getElementById('input').value = '';
        viewing = response.id;
        invocation = response;
        view.render();
      }, function(err) {
        displaySnackbar(err);
      });
    }
  });
}, false);
